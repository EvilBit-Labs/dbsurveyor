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

/// Command executor trait for implementing the command pattern
pub trait CommandExecutor: std::fmt::Debug {
    /// Execute the command and return a result message
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - Input file validation fails
    /// - Output path validation fails
    /// - Processing operation fails
    fn execute(&self) -> Result<String, ExecuteError>;
}

/// Process command implementation
#[derive(Debug)]
pub struct ProcessCommand {
    input: PathBuf,
    #[allow(dead_code)] // Will be used when processing functionality is implemented
    format: OutputFormat,
    output: Option<PathBuf>,
}

impl ProcessCommand {
    /// Create a new process command
    #[must_use]
    pub const fn new(input: PathBuf, format: OutputFormat, output: Option<PathBuf>) -> Self {
        Self {
            input,
            format,
            output,
        }
    }
}

impl CommandExecutor for ProcessCommand {
    fn execute(&self) -> Result<String, ExecuteError> {
        // Validate input file exists and is readable
        check_input_file_readable(&self.input)?;

        // Validate output path is writable if specified
        if let Some(output_path) = &self.output {
            check_output_path_writable(output_path)?;
        }

        let version = env!("CARGO_PKG_VERSION");
        Ok(format!(
            "dbsurveyor v{version}\nDatabase survey postprocessing and documentation tool\n✅ Input file validated: {}\n✅ Output path validated: {}\n⚠️  Processing functionality will be implemented in future tasks",
            self.input.display(),
            self.output
                .as_ref()
                .map_or_else(|| std::path::Path::new("stdout").display(), |p| p.display())
        ))
    }
}

/// Command factory for creating command executors
pub struct CommandFactory;

impl CommandFactory {
    /// Create a command executor from CLI arguments
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - No command is specified in the CLI arguments
    /// - The specified command is not supported
    pub fn create_command(cli: &Cli) -> Result<Box<dyn CommandExecutor>, ExecuteError> {
        match &cli.command {
            Some(Commands::Process {
                input,
                format,
                output,
            }) => {
                let command = ProcessCommand::new(input.clone(), format.clone(), output.clone());
                Ok(Box::new(command))
            }
            None => Err(ExecuteError::InvalidCommand {
                message: "No command specified".to_string(),
            }),
        }
    }
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

/// CLI execution error types
#[derive(Debug, thiserror::Error)]
pub enum ExecuteError {
    /// Input file does not exist or is not readable
    #[error("Input file does not exist or is not readable: {path}")]
    InputFileNotFound {
        /// The path that was not found
        path: String,
        /// The underlying IO error
        source: std::io::Error,
    },

    /// Output directory does not exist or is not writable
    #[error("Output directory does not exist or is not writable: {path}")]
    OutputDirectoryNotWritable {
        /// The path that is not writable
        path: String,
        /// The underlying IO error
        source: std::io::Error,
    },

    /// Output file cannot be created or written
    #[error("Output file cannot be created or written: {path}")]
    OutputFileNotWritable {
        /// The path that cannot be written
        path: String,
        /// The underlying IO error
        source: std::io::Error,
    },

    /// Processing operation failed
    #[error("Processing operation failed: {message}")]
    ProcessingFailed {
        /// Description of the processing failure
        message: String,
    },

    /// Invalid command or arguments
    #[error("Invalid command or arguments: {message}")]
    InvalidCommand {
        /// Description of the invalid command
        message: String,
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

    // Canonicalize base directory for consistent comparison
    let canonical_base_dir =
        base_dir
            .canonicalize()
            .map_err(|e| PathValidationError::CanonicalizationFailed {
                path: base_dir.to_string_lossy().to_string(),
                source: e,
            })?;

    // Resolve the path relative to base directory
    let resolved_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    };

    // For absolute paths, check if they're within the canonical base directory
    if path.is_absolute() && !resolved_path.starts_with(&canonical_base_dir) {
        return Err(PathValidationError::OutsideBaseDirectory {
            path: resolved_path.to_string_lossy().to_string(),
        });
    }

    // Safely resolve symlinks and validate path
    let canonical_path = if resolved_path.exists() {
        // Target exists, canonicalize and validate it stays within base directory
        let canonical = resolved_path.canonicalize().map_err(|e| {
            PathValidationError::CanonicalizationFailed {
                path: path_str.to_string(),
                source: e,
            }
        })?;

        // Canonicalize base directory for comparison
        let canonical_base_dir =
            base_dir
                .canonicalize()
                .map_err(|e| PathValidationError::CanonicalizationFailed {
                    path: base_dir.to_string_lossy().to_string(),
                    source: e,
                })?;

        // Validate canonical path is within canonical base directory
        if !canonical.starts_with(&canonical_base_dir) {
            return Err(PathValidationError::OutsideBaseDirectory {
                path: canonical.to_string_lossy().to_string(),
            });
        }
        canonical
    } else {
        // Target doesn't exist, canonicalize nearest existing parent directory
        let parent = resolved_path.parent().unwrap_or(base_dir);
        let canonical_parent =
            parent
                .canonicalize()
                .map_err(|e| PathValidationError::CanonicalizationFailed {
                    path: parent.to_string_lossy().to_string(),
                    source: e,
                })?;

        // Join with final component and validate
        let final_component = resolved_path
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new(""));
        let joined_path = canonical_parent.join(final_component);

        // Validate joined path stays within canonical base directory
        if !joined_path.starts_with(&canonical_base_dir) {
            return Err(PathValidationError::OutsideBaseDirectory {
                path: joined_path.to_string_lossy().to_string(),
            });
        }
        joined_path
    };

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

/// Check if a file exists and is readable
///
/// # Arguments
///
/// * `path` - The file path to check
///
/// # Returns
///
/// Returns `Ok(())` if the file exists and is readable, or an `ExecuteError` variant on failure
fn check_input_file_readable(path: &Path) -> Result<(), ExecuteError> {
    std::fs::metadata(path).map_err(|e| ExecuteError::InputFileNotFound {
        path: path.to_string_lossy().to_string(),
        source: e,
    })?;

    // Check if file is readable by attempting to open it
    std::fs::File::open(path).map_err(|e| ExecuteError::InputFileNotFound {
        path: path.to_string_lossy().to_string(),
        source: e,
    })?;

    Ok(())
}

/// Check if an output path is writable
///
/// # Arguments
///
/// * `path` - The output path to check
///
/// # Returns
///
/// Returns `Ok(())` if the path is writable, or an error if not
fn check_output_path_writable(path: &Path) -> Result<(), ExecuteError> {
    if path.exists() {
        // File exists, check if it's writable
        let metadata =
            std::fs::metadata(path).map_err(|e| ExecuteError::OutputFileNotWritable {
                path: path.to_string_lossy().to_string(),
                source: e,
            })?;

        if metadata.is_file() {
            // Try to open for writing without truncation
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .open(path)
                .map_err(|e| ExecuteError::OutputFileNotWritable {
                    path: path.to_string_lossy().to_string(),
                    source: e,
                })?;
        }
    } else {
        // File doesn't exist, check if parent directory is writable
        if let Some(parent) = path.parent() {
            if parent.exists() {
                // Check if parent directory is writable
                let metadata = std::fs::metadata(parent).map_err(|e| {
                    ExecuteError::OutputDirectoryNotWritable {
                        path: parent.to_string_lossy().to_string(),
                        source: e,
                    }
                })?;

                if metadata.permissions().readonly() {
                    return Err(ExecuteError::OutputDirectoryNotWritable {
                        path: parent.to_string_lossy().to_string(),
                        source: std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            "Directory is read-only",
                        ),
                    });
                }
                // Try to create a temporary file to test write permissions
                let temp_path = parent.join(".dbsurveyor_write_test");
                let write_result = std::fs::write(&temp_path, b"test");

                // Always attempt to clean up test file
                let _ = std::fs::remove_file(&temp_path);

                // Check write result after cleanup
                if let Err(e) = write_result {
                    return Err(ExecuteError::OutputDirectoryNotWritable {
                        path: parent.to_string_lossy().to_string(),
                        source: e,
                    });
                }
            } else {
                return Err(ExecuteError::OutputDirectoryNotWritable {
                    path: parent.to_string_lossy().to_string(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "Parent directory does not exist",
                    ),
                });
            }
        }
    }

    Ok(())
}

/// Execute CLI commands using the command pattern
///
/// # Arguments
///
/// * `cli` - The parsed CLI arguments
///
/// # Returns
///
/// Returns `Ok(String)` with the operation result, or an error if execution fails
///
/// # Errors
///
/// This function will return an error if:
/// - No command is specified
/// - Command creation fails
/// - Command execution fails
pub fn execute_cli(cli: &Cli) -> Result<String, ExecuteError> {
    if cli.command.is_some() {
        // Create command executor using factory
        let command = CommandFactory::create_command(cli)?;

        // Execute the command
        command.execute()
    } else {
        let version = env!("CARGO_PKG_VERSION");
        Ok(format!(
            "dbsurveyor v{version}\nDatabase survey postprocessing and documentation tool\nUse --help for available commands"
        ))
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
    #[allow(clippy::unwrap_used)]
    fn test_cli_process_command() {
        // Create a temporary test file
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("test.json");
        fs::write(&test_file, "test content").unwrap();

        let cli = Cli {
            command: Some(Commands::Process {
                input: test_file,
                format: OutputFormat::Markdown,
                output: Some(temp_dir.path().join("output.md")),
            }),
        };

        let result = execute_cli(&cli);
        assert!(result.is_ok());

        if let Ok(output) = result {
            assert!(output.contains("dbsurveyor v"));
            assert!(output.contains("Database survey postprocessing and documentation tool"));
            assert!(
                output.contains("Processing functionality will be implemented in future tasks")
            );
        }
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_command_factory_create_process_command() {
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("test.json");
        fs::write(&test_file, "test content").unwrap();

        let cli = Cli {
            command: Some(Commands::Process {
                input: test_file,
                format: OutputFormat::Json,
                output: None,
            }),
        };

        let command = CommandFactory::create_command(&cli);
        assert!(command.is_ok());

        let result = command.unwrap().execute();
        assert!(result.is_ok());
    }

    #[test]
    #[allow(clippy::unwrap_used, clippy::panic)]
    fn test_command_factory_no_command() {
        let cli = Cli { command: None };

        let command = CommandFactory::create_command(&cli);
        assert!(command.is_err());

        match command.unwrap_err() {
            ExecuteError::InvalidCommand { message } => {
                assert_eq!(message, "No command specified");
            }
            _ => panic!("Expected InvalidCommand error"),
        }
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_process_command_executor() {
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("test.json");
        fs::write(&test_file, "test content").unwrap();

        let command = ProcessCommand::new(
            test_file,
            OutputFormat::Markdown,
            Some(temp_dir.path().join("output.md")),
        );

        let result = command.execute();
        assert!(result.is_ok());

        if let Ok(output) = result {
            assert!(output.contains("dbsurveyor v"));
            assert!(output.contains("Input file validated"));
            assert!(output.contains("Output path validated"));
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
    #[allow(clippy::unwrap_used)]
    fn test_validate_file_path_success() {
        let temp_dir = tempdir().unwrap();
        let base_dir = temp_dir.path();

        // Create a subdirectory for the test file (not in base_dir directly)
        let sub_dir = base_dir.join("subdir");
        fs::create_dir(&sub_dir).unwrap();
        let test_file = sub_dir.join("test.json");
        fs::write(&test_file, "test content").unwrap();

        // Use relative path for validation (relative to base_dir)
        let relative_path = PathBuf::from("subdir").join("test.json");
        let result = validate_file_path(&relative_path, base_dir);
        if let Err(ref e) = result {
            println!("Validation failed: {e:?}");
        }
        assert!(result.is_ok());

        let canonical_path = result.unwrap();
        let canonical_base_dir = base_dir
            .canonicalize()
            .unwrap_or_else(|_| base_dir.to_path_buf());
        assert!(canonical_path.starts_with(&canonical_base_dir));
    }

    #[test]
    #[allow(clippy::unwrap_used, clippy::panic)]
    fn test_validate_file_path_directory_traversal() {
        let temp_dir = tempdir().unwrap();
        let base_dir = temp_dir.path();

        let malicious_path = PathBuf::from("../../../etc/passwd");
        let result = validate_file_path(&malicious_path, base_dir);

        assert!(result.is_err());
        match result {
            Err(PathValidationError::DirectoryTraversal { path }) => {
                assert!(path.contains(".."));
            }
            _ => unreachable!("Expected DirectoryTraversal error"),
        }
    }

    #[test]
    #[allow(clippy::unwrap_used, clippy::panic)]
    fn test_validate_file_path_leading_separator_relative() {
        let temp_dir = tempdir().unwrap();
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
            _ => unreachable!("Expected OutsideBaseDirectory error"),
        }
    }

    #[test]
    #[allow(clippy::unwrap_used, clippy::panic)]
    fn test_validate_file_path_leading_separator() {
        let temp_dir = tempdir().unwrap();
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
            _ => unreachable!("Expected OutsideBaseDirectory error"),
        }
    }

    #[test]
    #[allow(clippy::unwrap_used, clippy::panic)]
    fn test_validate_file_path_outside_base_directory() {
        let temp_dir = tempdir().unwrap();
        let base_dir = temp_dir.path();

        // Create a file outside the base directory
        let outside_dir = tempfile::tempdir().unwrap();
        let outside_file = outside_dir.path().join("test.json");
        fs::write(&outside_file, "test content").unwrap();

        let result = validate_file_path(&outside_file, base_dir);

        assert!(result.is_err());
        match result {
            Err(PathValidationError::OutsideBaseDirectory { path }) => {
                assert!(!path.starts_with(&base_dir.to_string_lossy().to_string()));
            }
            _ => unreachable!("Expected OutsideBaseDirectory error"),
        }
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_validate_cli_paths_success() {
        let temp_dir = tempdir().unwrap();
        let base_dir = temp_dir.path();

        // Create test files
        let input_file = base_dir.join("input.json");
        let output_file = base_dir.join("output.md");
        fs::write(&input_file, "test input").unwrap();
        fs::write(&output_file, "test output").unwrap();

        // Change to the temp directory for this test
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(base_dir).unwrap();

        let cli = Cli {
            command: Some(Commands::Process {
                input: PathBuf::from("input.json"),
                format: OutputFormat::Markdown,
                output: Some(PathBuf::from("output.md")),
            }),
        };

        let result = validate_cli_paths(&cli);
        assert!(result.is_ok());

        let validated_cli = result.unwrap();
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
        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    #[allow(clippy::unwrap_used, clippy::panic)]
    fn test_validate_cli_paths_failure() {
        // Create temp dir but don't use it in this test
        let _temp_dir = tempdir().unwrap();

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
            _ => unreachable!("Expected DirectoryTraversal error"),
        }
    }
}
