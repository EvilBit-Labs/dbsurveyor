//! Error types with comprehensive credential sanitization.
//!
//! All error types in this module ensure that database credentials, connection
//! strings, and other sensitive information are never exposed in error messages,
//! logs, or any output format.

use thiserror::Error;

/// Main error type for DBSurveyor operations.
///
/// # Security
/// All error messages are sanitized to prevent credential leakage.
/// Connection strings and passwords are never included in error output.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DbSurveyorError {
    /// Database connection failed (credentials sanitized)
    #[error("Database connection failed: {context}")]
    Connection {
        context: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Schema collection operation failed
    #[error("Schema collection failed: {context}")]
    Collection {
        context: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Encryption or decryption operation failed
    #[cfg(feature = "encryption")]
    #[error("Encryption operation failed: {context}")]
    Encryption {
        context: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Configuration or validation error
    #[error("Configuration error: {message}")]
    Configuration { message: String },

    /// Insufficient privileges for database operation
    #[error("Insufficient privileges: {required}")]
    InsufficientPrivileges { required: String },

    /// Query timeout or execution failure
    #[error("Query execution failed: {context}")]
    QueryExecution { context: String },

    /// Unsupported database feature or operation
    #[error("Unsupported operation: {feature} not supported for {database_type}")]
    UnsupportedFeature {
        feature: String,
        database_type: String,
    },

    /// Connection acquisition timed out
    #[error("Connection timeout: {context} (timeout: {timeout:?})")]
    ConnectionTimeout {
        context: String,
        timeout: std::time::Duration,
    },

    /// I/O operation failed
    #[error("I/O operation failed: {context}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },

    /// Serialization or deserialization failed
    #[error("Serialization failed: {context}")]
    Serialization {
        context: String,
        #[source]
        source: serde_json::Error,
    },
}

/// Convenience type alias for Results with DbSurveyorError
pub type Result<T> = std::result::Result<T, DbSurveyorError>;

/// Safely redacts database URLs for logging and error messages.
///
/// This function ensures that passwords in connection strings are never
/// exposed in logs, error messages, or any output.
///
/// # Arguments
///
/// * `url` - Database connection URL that may contain credentials
///
/// # Returns
///
/// Returns a sanitized string with passwords masked as "****"
///
/// # Example
///
/// ```rust
/// use dbsurveyor_core::error::redact_database_url;
///
/// let sanitized = redact_database_url("postgres://user:secret@localhost/db");
/// assert_eq!(sanitized, "postgres://user:****@localhost/db");
/// assert!(!sanitized.contains("secret"));
/// ```
pub fn redact_database_url(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(mut parsed_url) => {
            if parsed_url.password().is_some() {
                let _ = parsed_url.set_password(Some("****"));
            }
            parsed_url.to_string()
        }
        Err(_) => "<redacted>".to_string(),
    }
}

impl DbSurveyorError {
    /// Creates a connection error with sanitized context
    pub fn connection_failed<E>(error: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Connection {
            context: "Database connection failed".to_string(),
            source: Box::new(error),
        }
    }

    /// Creates a parsing error for database column extraction
    ///
    /// This is a convenience method for the common pattern of parsing
    /// values from database result rows.
    ///
    /// # Arguments
    /// * `field_name` - Name of the field being parsed
    /// * `table_context` - Optional table context for better error messages
    /// * `error` - The underlying parsing error
    pub fn parse_field<E>(field_name: &str, table_context: Option<&str>, error: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        let context = match table_context {
            Some(table) => format!(
                "Failed to parse field '{}' from result for table '{}'",
                field_name, table
            ),
            None => format!(
                "Failed to parse field '{}' from database result",
                field_name
            ),
        };
        Self::Collection {
            context,
            source: Box::new(error),
        }
    }

    /// Creates a collection error with context
    pub fn collection_failed<E>(context: impl Into<String>, error: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Collection {
            context: context.into(),
            source: Box::new(error),
        }
    }

    /// Creates a configuration error
    pub fn configuration(message: impl Into<String>) -> Self {
        Self::Configuration {
            message: message.into(),
        }
    }

    /// Creates an insufficient privileges error
    pub fn insufficient_privileges(required: impl Into<String>) -> Self {
        Self::InsufficientPrivileges {
            required: required.into(),
        }
    }

    /// Creates a query execution error
    pub fn query_failed(context: impl Into<String>) -> Self {
        Self::QueryExecution {
            context: context.into(),
        }
    }

    /// Creates an unsupported feature error
    pub fn unsupported_feature(
        feature: impl Into<String>,
        database_type: impl Into<String>,
    ) -> Self {
        Self::UnsupportedFeature {
            feature: feature.into(),
            database_type: database_type.into(),
        }
    }

    /// Creates a connection timeout error
    pub fn connection_timeout(context: impl Into<String>, timeout: std::time::Duration) -> Self {
        Self::ConnectionTimeout {
            context: context.into(),
            timeout,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_database_url() {
        let url = "postgres://user:secret@localhost/db";
        let redacted = redact_database_url(url);

        assert!(!redacted.contains("secret"));
        assert!(!redacted.contains("user:secret"));
        assert!(redacted.contains("user:****"));
        assert!(redacted.contains("localhost/db"));
    }

    #[test]
    fn test_redact_database_url_no_password() {
        let url = "postgres://user@localhost/db";
        let redacted = redact_database_url(url);

        assert_eq!(redacted, "postgres://user@localhost/db");
    }

    #[test]
    fn test_redact_invalid_url() {
        let invalid_url = "not-a-url";
        let redacted = redact_database_url(invalid_url);

        assert_eq!(redacted, "<redacted>");
    }

    #[test]
    fn test_error_creation() {
        let error = DbSurveyorError::configuration("Invalid database type");
        assert!(error.to_string().contains("Invalid database type"));

        let error = DbSurveyorError::insufficient_privileges("SELECT on schema");
        assert!(error.to_string().contains("SELECT on schema"));
    }

    #[test]
    fn test_connection_failed_error() {
        let source = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        let error = DbSurveyorError::connection_failed(source);
        let msg = error.to_string();
        assert!(msg.contains("Database connection failed"));
        assert!(!msg.contains("refused"));
    }

    #[test]
    fn test_parse_field_with_table_context() {
        let source = std::io::Error::new(std::io::ErrorKind::InvalidData, "bad data");
        let error = DbSurveyorError::parse_field("column_name", Some("users"), source);
        let msg = error.to_string();
        assert!(msg.contains("column_name"));
        assert!(msg.contains("users"));
    }

    #[test]
    fn test_parse_field_without_table_context() {
        let source = std::io::Error::new(std::io::ErrorKind::InvalidData, "bad");
        let error = DbSurveyorError::parse_field("version", None, source);
        let msg = error.to_string();
        assert!(msg.contains("version"));
        assert!(msg.contains("database result"));
    }

    #[test]
    fn test_collection_failed_error() {
        let source = std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout");
        let error = DbSurveyorError::collection_failed("Schema enumeration", source);
        let msg = error.to_string();
        assert!(msg.contains("Schema enumeration"));
    }

    #[test]
    fn test_query_failed_error() {
        let error = DbSurveyorError::query_failed("SELECT failed on pg_catalog");
        let msg = error.to_string();
        assert!(msg.contains("SELECT failed on pg_catalog"));
    }

    #[test]
    fn test_unsupported_feature_error() {
        let error =
            DbSurveyorError::unsupported_feature("multi-database", "SQLite");
        let msg = error.to_string();
        assert!(msg.contains("multi-database"));
        assert!(msg.contains("SQLite"));
    }

    #[test]
    fn test_connection_timeout_error() {
        let error = DbSurveyorError::connection_timeout(
            "Pool acquisition",
            std::time::Duration::from_secs(30),
        );
        let msg = error.to_string();
        assert!(msg.contains("Pool acquisition"));
        assert!(msg.contains("30"));
    }

    #[test]
    fn test_redact_mysql_url() {
        let url = "mysql://root:mysecret@db.example.com:3306/mydb";
        let redacted = redact_database_url(url);
        assert!(!redacted.contains("mysecret"));
        assert!(redacted.contains("root:****"));
    }

    #[test]
    fn test_redact_mongodb_url() {
        let url = "mongodb://admin:p%40ss@mongo.local:27017/admin";
        let redacted = redact_database_url(url);
        assert!(!redacted.contains("p%40ss"));
        assert!(redacted.contains("****"));
    }

    #[test]
    fn test_redact_url_with_special_chars_in_password() {
        let url = "postgres://user:p%40ss%3Aw0rd@localhost/db";
        let redacted = redact_database_url(url);
        assert!(!redacted.contains("p%40ss"));
        assert!(!redacted.contains("w0rd"));
        assert!(redacted.contains("****"));
    }

    #[test]
    fn test_error_messages_never_contain_credentials() {
        let connection_url = "postgres://admin:supersecret@prod.db:5432/app";
        let source = std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "connection refused",
        );
        let error = DbSurveyorError::connection_failed(source);
        let msg = format!("{error}");
        assert!(!msg.contains("supersecret"));
        assert!(!msg.contains("admin:supersecret"));
        assert!(!msg.contains(connection_url));
    }
}
