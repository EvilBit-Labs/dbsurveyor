//! SQLite connection handling.
//!
//! SQLite uses file-based databases, so connection handling is simpler than
//! pooled connections. This module provides connection creation and validation.
//!
//! # Connection Modes
//! - File-based: `sqlite:///path/to/database.db` or `sqlite://./relative.db`
//! - In-memory: `sqlite::memory:` or `:memory:`
//!
//! # Security Features
//! - Opens databases in read-only mode by default
//! - No network access required
//! - File path validation

use super::{ConnectionConfig, SqliteAdapter};
use crate::Result;
use sqlx::SqlitePool;
use url::Url;

impl SqliteAdapter {
    /// Creates a new SQLite adapter from a connection string.
    ///
    /// # Arguments
    /// * `connection_string` - SQLite connection URL or file path
    ///
    /// # Connection String Formats
    /// - `sqlite:///path/to/database.db` - Absolute file path
    /// - `sqlite://./relative/path.db` - Relative file path
    /// - `sqlite::memory:` or `:memory:` - In-memory database
    ///
    /// # Security
    /// - Opens database in read-only mode for schema collection
    /// - Validates connection string format
    ///
    /// # Errors
    /// Returns error if:
    /// - Connection string format is invalid
    /// - Database file does not exist (for file-based DBs)
    /// - Database cannot be opened
    pub async fn new(connection_string: &str) -> Result<Self> {
        let config = parse_sqlite_connection_config(connection_string)?;
        let pool = create_sqlite_connection(connection_string, &config).await?;

        Ok(Self {
            pool,
            config,
            connection_string: connection_string.to_string(),
        })
    }

    /// Creates a new SQLite adapter with custom configuration.
    ///
    /// # Arguments
    /// * `connection_string` - SQLite connection URL or file path
    /// * `config` - Custom connection configuration
    pub async fn with_config(connection_string: &str, config: ConnectionConfig) -> Result<Self> {
        config.validate()?;
        validate_sqlite_connection_string(connection_string)?;
        let pool = create_sqlite_connection(connection_string, &config).await?;

        Ok(Self {
            pool,
            config,
            connection_string: connection_string.to_string(),
        })
    }

    /// Checks if the connection is to an in-memory database.
    pub fn is_in_memory(&self) -> bool {
        self.connection_string.contains(":memory:")
            || self.connection_string.contains("mode=memory")
    }

    /// Gets the database file path if using a file-based database.
    pub fn database_path(&self) -> Option<String> {
        if self.is_in_memory() {
            return None;
        }

        // Try to extract path from connection string
        if let Some(stripped) = self.connection_string.strip_prefix("sqlite://") {
            let path = stripped.split('?').next().unwrap_or(stripped);
            if !path.is_empty() && path != ":memory:" {
                return Some(path.to_string());
            }
        } else if self.connection_string.ends_with(".db")
            || self.connection_string.ends_with(".sqlite")
            || self.connection_string.ends_with(".sqlite3")
        {
            return Some(self.connection_string.clone());
        }

        None
    }

    /// Closes the connection gracefully.
    pub async fn close(&self) {
        self.pool.close().await;
    }

    /// Checks if the connection is healthy.
    pub async fn is_healthy(&self) -> bool {
        match sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await
        {
            Ok(result) => result == 1,
            Err(_) => false,
        }
    }
}

/// Parses SQLite connection string to extract configuration parameters.
///
/// # Arguments
/// * `connection_string` - SQLite connection URL or file path
///
/// # Returns
/// Validated connection configuration
pub fn parse_sqlite_connection_config(connection_string: &str) -> Result<ConnectionConfig> {
    validate_sqlite_connection_string(connection_string)?;

    // Determine database name from path
    let database_name = extract_database_name(connection_string);

    let mut config = ConnectionConfig::new("localhost".to_string());
    config = config.with_database(database_name);

    // SQLite doesn't use ports, but we set a dummy value for consistency
    config.port = None;

    // SQLite uses a single connection (no pooling needed)
    config.max_connections = 1;
    config.min_idle_connections = 0;

    Ok(config)
}

/// Validates SQLite connection string format.
///
/// # Arguments
/// * `connection_string` - SQLite connection URL to validate
///
/// # Errors
/// Returns error if connection string is invalid
pub fn validate_sqlite_connection_string(connection_string: &str) -> Result<()> {
    // Handle in-memory shorthand
    if connection_string == ":memory:" {
        return Ok(());
    }

    // Handle file path directly (e.g., "/path/to/db.sqlite")
    if connection_string.ends_with(".db")
        || connection_string.ends_with(".sqlite")
        || connection_string.ends_with(".sqlite3")
    {
        return Ok(());
    }

    // Handle sqlite:// URL format
    if connection_string.starts_with("sqlite:") {
        // sqlite::memory: is valid
        if connection_string.contains(":memory:") || connection_string.contains("mode=memory") {
            return Ok(());
        }

        // Try to parse as URL
        if let Ok(url) = Url::parse(connection_string) {
            if url.scheme() != "sqlite" {
                return Err(crate::error::DbSurveyorError::configuration(
                    "Connection string must use sqlite:// scheme",
                ));
            }
            return Ok(());
        }

        // Check for valid path-like structure
        if connection_string.starts_with("sqlite://") || connection_string.starts_with("sqlite:///")
        {
            return Ok(());
        }
    }

    Err(crate::error::DbSurveyorError::configuration(
        "Invalid SQLite connection string format: expected sqlite:// URL, file path, or :memory:",
    ))
}

/// Extracts database name from connection string.
fn extract_database_name(connection_string: &str) -> String {
    if connection_string == ":memory:" || connection_string.contains(":memory:") {
        return ":memory:".to_string();
    }

    // Try to extract filename from path
    if let Some(stripped) = connection_string.strip_prefix("sqlite://") {
        let path = stripped.split('?').next().unwrap_or(stripped);
        if let Some(filename) = path.rsplit('/').next()
            && !filename.is_empty()
            && filename != ":memory:"
        {
            return filename.to_string();
        }
    }

    // Direct file path
    if let Some(filename) = connection_string.rsplit('/').next()
        && !filename.is_empty()
    {
        return filename.to_string();
    }

    "main".to_string()
}

/// Creates a SQLite connection with proper configuration.
async fn create_sqlite_connection(
    connection_string: &str,
    config: &ConnectionConfig,
) -> Result<SqlitePool> {
    use sqlx::sqlite::SqliteConnectOptions;
    use std::str::FromStr;

    // Normalize connection string
    let normalized = normalize_connection_string(connection_string);

    // Parse connection options
    let mut options = SqliteConnectOptions::from_str(&normalized).map_err(|e| {
        crate::error::DbSurveyorError::configuration(format!(
            "Invalid SQLite connection string: {}",
            e
        ))
    })?;

    // Configure for read-only access if requested
    if config.read_only {
        options = options.read_only(true);
    }

    // Create pool (SQLite pools are typically single-connection)
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(config.max_connections.max(1))
        .acquire_timeout(config.connect_timeout)
        .idle_timeout(config.idle_timeout)
        .connect_with(options)
        .await
        .map_err(|e| {
            crate::error::DbSurveyorError::collection_failed("Failed to open SQLite database", e)
        })?;

    Ok(pool)
}

/// Normalizes connection string to SQLite URL format.
fn normalize_connection_string(connection_string: &str) -> String {
    if connection_string == ":memory:" {
        return "sqlite::memory:".to_string();
    }

    if connection_string.starts_with("sqlite:") {
        return connection_string.to_string();
    }

    // Convert file path to sqlite:// URL
    format!("sqlite://{}", connection_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_sqlite_connection_string_memory() {
        assert!(validate_sqlite_connection_string(":memory:").is_ok());
        assert!(validate_sqlite_connection_string("sqlite::memory:").is_ok());
        assert!(validate_sqlite_connection_string("sqlite://:memory:").is_ok());
    }

    #[test]
    fn test_validate_sqlite_connection_string_file() {
        assert!(validate_sqlite_connection_string("sqlite:///path/to/db.sqlite").is_ok());
        assert!(validate_sqlite_connection_string("sqlite://./test.db").is_ok());
        assert!(validate_sqlite_connection_string("/path/to/database.db").is_ok());
        assert!(validate_sqlite_connection_string("./local.sqlite").is_ok());
        assert!(validate_sqlite_connection_string("data.sqlite3").is_ok());
    }

    #[test]
    fn test_validate_sqlite_connection_string_invalid() {
        assert!(validate_sqlite_connection_string("postgres://localhost/db").is_err());
        assert!(validate_sqlite_connection_string("mysql://localhost/db").is_err());
        assert!(validate_sqlite_connection_string("invalid").is_err());
    }

    #[test]
    fn test_extract_database_name() {
        assert_eq!(extract_database_name(":memory:"), ":memory:");
        assert_eq!(
            extract_database_name("sqlite:///path/to/mydb.sqlite"),
            "mydb.sqlite"
        );
        assert_eq!(extract_database_name("sqlite://./test.db"), "test.db");
        assert_eq!(extract_database_name("/var/data/app.db"), "app.db");
    }

    #[test]
    fn test_normalize_connection_string() {
        assert_eq!(normalize_connection_string(":memory:"), "sqlite::memory:");
        assert_eq!(
            normalize_connection_string("sqlite:///path/db.sqlite"),
            "sqlite:///path/db.sqlite"
        );
        assert_eq!(
            normalize_connection_string("/path/to/db.sqlite"),
            "sqlite:///path/to/db.sqlite"
        );
    }

    #[test]
    fn test_parse_sqlite_connection_config() {
        let config = parse_sqlite_connection_config("sqlite:///path/to/test.db").unwrap();
        assert_eq!(config.database, Some("test.db".to_string()));
        assert_eq!(config.max_connections, 1);
        assert_eq!(config.port, None);

        let config = parse_sqlite_connection_config(":memory:").unwrap();
        assert_eq!(config.database, Some(":memory:".to_string()));
    }
}
